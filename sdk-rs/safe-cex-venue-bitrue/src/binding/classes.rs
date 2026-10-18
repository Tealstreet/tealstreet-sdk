//! Constructor and lexical member binding for the ported Bitrue class.
use super::{Rt, Shared, host::text};
use crate::{collection_reads::Callback, composition::CompositionHost};
use safe_cex_exchange::js_runtime::{
    base_exchange, globals, library,
    promise::{JsFuture, Promise, await_js},
    scope::scope_of,
    value::{Js, Thrown, arg, from_json, from_typed, to_json},
};
use std::rc::{Rc, Weak};

pub(super) fn super_member(rt: &Rt, receiver: &Js, name: &str) -> Result<Js, Thrown> {
    rt.shared
        .extra
        .supers
        .borrow()
        .iter()
        .find(|(r, _)| r.same_reference(receiver))
        .map(|(_, s)| s.clone())
        .ok_or_else(|| Thrown::error("Error", "Missing Bitrue superclass"))?
        .get(name)
}
fn coercion(e: safe_cex_num::JsCoercionError) -> Thrown {
    Thrown::error(e.name, e.message)
}
fn exchange_error(e: safe_cex_exchange::runtime::ExchangeError) -> Thrown {
    let response = e.retained_response_input();
    let thrown = Thrown::from(e);
    if let Some(response) = response {
        let _ = thrown.value.define(
            "response",
            safe_cex_exchange::js_runtime::value::from_retained_input(response),
        );
    }
    thrown
}
pub(super) fn install(g: &Js, weak: Weak<Shared>) -> Result<(), Thrown> {
    globals::install_builtins(g, weak.clone());
    globals::install_enums(g);
    library::install_library(g, weak.clone());
    let table = |entries: &[(&str, &str)]| {
        Js::object(entries.iter().map(|(k, v)| (*k, Js::str(v))).collect())
    };
    g.define("ENDPOINTS", table(&crate::types::ENDPOINTS))?;
    g.define("BASE_WS_URL", table(&crate::types::BASE_WS_URL))?;
    g.define("INTERVAL", table(&crate::types::INTERVAL))?;
    macro_rules! enums {
        ($name:literal,$rows:expr) => {
            g.define(
                $name,
                Js::object(
                    $rows
                        .iter()
                        .map(|(k, v)| Ok((*k, from_typed(v)?)))
                        .collect::<Result<_, Thrown>>()?,
                ),
            )?;
        };
    }
    enums!("ORDER_TYPE", crate::types::ORDER_TYPE);
    enums!("ORDER_SIDE", crate::types::ORDER_SIDE);
    enums!("ORDER_STATUS", crate::types::ORDER_STATUS);
    g.define(
        "listenKeyRetryDelays",
        Js::array(
            crate::private_auth::RETRY_DELAYS
                .iter()
                .map(|x| Js::Number(*x))
                .collect(),
        ),
    )?;
    g.define(
        "rateLimit",
        Js::function("rateLimit", |_, a| {
            let client = arg(&a, 0);
            client.call_method("__configureRateLimit", vec![arg(&a, 1)])?;
            Ok(client)
        }),
    )?;
    let w = weak.clone();
    g.define(
        "createAPI",
        Js::function("createAPI", move |_, a| {
            let rt = scope_of(&w)?;
            crate::private_auth::create_api(&rt, arg(&a, 0), arg(&a, 1))
        }),
    )?;
    g.define(
        "bigIntAsStringJsonParse",
        Js::function("bigIntAsStringJsonParse", |_, a| {
            let value = arg(&a, 0);
            if !value.is_string() {
                return Ok(value);
            }
            Ok(safe_cex_exchange::js_runtime::value::from_retained_input(
                safe_cex_transport::how::big_int_as_string_json_parse(&text(&value)?),
            ))
        }),
    )?;
    g.define(
        "jsonParse",
        Js::function("jsonParse", |_, a| {
            // jsonParse catches both ToString and parsing errors.
            Ok(text(&arg(&a, 0))
                .map(|s| {
                    safe_cex_exchange::js_runtime::value::from_retained_input(
                        safe_cex_transport::how::json_parse(&s),
                    )
                })
                .unwrap_or(Js::Null))
        }),
    )?;
    let w = weak.clone();
    g.define(
        "readBlob",
        Js::function("readBlob", move |_, a| {
            let h = scope_of(&w)?;
            let buffer = arg(&a, 0).call_method("arrayBuffer", vec![]);
            Ok(h.clone().spawn(Box::pin(async move {
                let bytes = await_js(&h.exchange_runtime(), buffer?)
                    .await?
                    .get("bytes")?
                    .array_values()
                    .ok_or_else(|| Thrown::type_error("ArrayBuffer bytes are unavailable"))?
                    .iter()
                    .map(|v| v.to_number().map(|n| n as u8))
                    .collect::<Result<Vec<_>, _>>()?;
                safe_cex_store::read_blob::read_blob(async { Ok(bytes) })
                    .await
                    .map(|s| Js::str(&s))
                    .map_err(|e| Thrown::error("Error", &e))
            })))
        }),
    )?;
    g.define(
        "reverseWsSymbol",
        Js::function("reverseWsSymbol", |_, a| {
            Ok(Js::str(&crate::utils::reverse_ws_symbol(&text(&arg(
                &a, 0,
            ))?)))
        }),
    )?;
    Ok(())
}
impl CompositionHost for Rt {
    fn super_exchange(&self, args: Vec<Js>) -> Result<Js, Thrown> {
        let parts = self
            .shared
            .extra
            .parts
            .borrow_mut()
            .take()
            .ok_or_else(|| Thrown::error("Error", "Bitrue base already constructed"))?;
        let (r, base) = base_exchange::create(
            &self.shared.sockets,
            parts,
            arg(&args, 0),
            arg(&args, 1),
            arg(&args, 2),
            |_| Ok(()),
        )?;
        self.shared
            .base
            .set(base)
            .map_err(|_| Thrown::error("Error", "Bitrue base already installed"))?;
        self.shared
            .extra
            .supers
            .borrow_mut()
            .push((r.clone(), Js::object(vec![("dispose", r.get("dispose")?)])));
        Ok(r)
    }
    fn super_public(&self, args: Vec<Js>) -> Result<Js, Thrown> {
        super::sockets::base(self, arg(&args, 0))
    }
    fn define(&self, r: Js, key: &str, v: Js) -> Result<(), Thrown> {
        r.define(key, v)
    }
    fn new_set(&self) -> Result<Js, Thrown> {
        Ok(Js::new_set())
    }
    fn new_map(&self) -> Result<Js, Thrown> {
        Ok(Js::new_map())
    }
    fn rule(&self, label: &str, pattern: &str) -> Result<Js, Thrown> {
        let regex = safe_cex_exchange::js_runtime::regexp::regexp(
            &pattern.encode_utf16().collect::<Vec<_>>(),
            &[],
        )?;
        Ok(Js::object(vec![
            ("label", Js::str(label)),
            (
                "validate",
                Js::function("validate", move |_, a| {
                    regex.call_method("test", vec![arg(&a, 0)])
                }),
            ),
        ]))
    }
    fn grafana(&self, which: &str) -> Result<Js, Thrown> {
        Ok(Js::array(
            if which == "every" {
                crate::types::GRAFANA_ENDPOINTS_EVERY.as_slice()
            } else {
                crate::types::GRAFANA_ENDPOINTS_PERIODIC.as_slice()
            }
            .iter()
            .map(|v| Js::str(v))
            .collect(),
        ))
    }
    fn binding(&self, name: &'static str) -> Result<Js, Thrown> {
        self.global(name)
    }
    fn construct_socket(&self, which: &'static str, r: Js) -> Result<Js, Thrown> {
        let socket = if which == "public" {
            crate::composition::construct_public(self, vec![r])?
        } else {
            let s = crate::private_auth::construct(self, vec![r])?;
            s.define(
                "dispose",
                super::sockets::private_member(self, s.clone(), "dispose"),
            )?;
            s
        };
        Ok(socket)
    }
    fn member(&self, name: &'static str, _target: &'static str, r: Js) -> Js {
        if crate::composition::PUBLIC_MEMBERS
            .iter()
            .any(|(k, _)| *k == name)
            && !crate::composition::EXCHANGE_MEMBERS
                .iter()
                .any(|(k, _)| *k == name)
        {
            return super::sockets::public_member(self, r, name);
        }
        // Exchange and public class share a few names. A socket has a parent;
        // the exchange instead owns the options/store and the two socket fields.
        if r.has_own("parent") {
            super::sockets::public_member(self, r, name)
        } else {
            member(self, r, name)
        }
    }
    fn named_function(&self, name: &'static str, f: Callback<Self>) -> Js {
        Js::function(name, move |_, a| f(a))
    }
}

fn member(rt: &Rt, r: Js, name: &'static str) -> Js {
    let weak = Rc::downgrade(&rt.shared);
    Js::function(name, move |_, args| {
        let h = scope_of(&weak)?.with_this(r.clone());
        let x = |i| arg(&args, i);
        match name {
            "isSuccessCode" => {
                let v = if x(0).is_undefined() {
                    None
                } else {
                    Some(to_json(&x(0))?)
                };
                return Ok(Js::Bool(crate::read_helpers::is_success_code(v.as_ref())));
            }
            "normalizeTimestampMs" => {
                return Ok(crate::read_helpers::normalize_timestamp_ms(
                    if let Js::Number(n) = x(0) {
                        Some(n)
                    } else {
                        None
                    },
                )
                .map_or(Js::Undefined, Js::Number));
            }
            "normalizeTradeSymbol" => {
                let v = if x(0).is_undefined() {
                    None
                } else {
                    Some(to_json(&x(0))?)
                };
                return crate::read_helpers::normalize_trade_symbol(v.as_ref())
                    .map(|v| Js::str(&v))
                    .map_err(coercion);
            }
            "parseMaxLeverageFromErrorMessage" => {
                let v = if x(0).is_nullish() {
                    None
                } else {
                    Some(text(&x(0))?)
                };
                return Ok(crate::read_helpers::parse_max_leverage_from_error_message(
                    v.as_deref(),
                )
                .map_or(Js::Null, Js::Number));
            }
            "parseMaxLeverageFromBracketResponse" => {
                let v = if x(0).is_undefined() {
                    None
                } else {
                    Some(to_json(&x(0))?)
                };
                let symbol = if x(1).is_undefined() {
                    None
                } else {
                    Some(text(&x(1))?)
                };
                return crate::read_helpers::parse_max_leverage_from_bracket_response(
                    v.as_ref(),
                    symbol.as_deref(),
                )
                .map(|v| v.map_or(Js::Null, Js::Number))
                .map_err(coercion);
            }
            "setMarketMaxLeverage" => {
                crate::read_helpers::set_market_max_leverage(
                    h.shared.store.as_ref(),
                    &text(&x(0))?,
                    x(1).to_number()?,
                )
                .map_err(exchange_error)?;
                return Ok(Js::Undefined);
            }
            "pickTradeOrderId" => {
                let v = to_json(&x(0))?;
                return crate::read_helpers::pick_trade_order_id(
                    v.as_object()
                        .ok_or_else(|| Thrown::type_error("trade entry must be an object"))?,
                )
                .map(|v| v.map_or(Js::Undefined, |v| Js::str(&v)))
                .map_err(coercion);
            }
            "resolveTradeQty" => {
                let v = to_json(&x(0))?;
                return crate::read_helpers::resolve_trade_qty(
                    v.as_object()
                        .ok_or_else(|| Thrown::type_error("trade entry must be an object"))?,
                    h.shared.store.as_ref(),
                    &text(&x(1))?,
                    x(2).to_number()?,
                )
                .map(Js::Number)
                .map_err(coercion);
            }
            "parseBalance" => {
                if !x(0).truthy() {
                    return r.get("store")?.get("balance");
                }
                let v = to_json(&x(0))?;
                let balance = h.shared.store.balance();
                return from_typed(
                    crate::balance::parse_balance(Some(&v), &balance, |message| {
                        h.exchange_runtime().console("warn", message)
                    })
                    .map_err(coercion)?
                    .as_ref(),
                );
            }
            "getPositionAccounts" => {
                let v = if x(0).is_undefined() {
                    None
                } else {
                    Some(to_json(&x(0))?)
                };
                return crate::account_helpers::get_position_accounts(v.as_ref())
                    .map(|v| {
                        v.map_or(Js::Undefined, |v| {
                            Js::array(v.into_iter().map(from_json).collect())
                        })
                    })
                    .map_err(coercion);
            }
            "getAccountPositionSnapshots" => {
                let v = if x(0).is_undefined() {
                    None
                } else {
                    Some(to_json(&x(0))?)
                };
                return crate::account_helpers::get_account_position_snapshots(v.as_ref())
                    .map(|rows| {
                        rows.map_or(Js::Undefined, |rows| {
                            Js::array(
                                rows.into_iter()
                                    .map(|row| {
                                        Js::object(vec![
                                            ("symbol", Js::str(row.symbol)),
                                            ("side", from_typed(&row.side).expect("enum")),
                                            ("leverageLevel", from_json(row.leverage_level)),
                                            ("positionType", from_json(row.position_type)),
                                        ])
                                    })
                                    .collect(),
                            )
                        })
                    })
                    .map_err(coercion);
            }
            "dispose" => return crate::runtime_lifecycle::dispose(&h, r.clone()),
            "queueFastPrivateRefresh" => {
                return crate::runtime_lifecycle::queue_refresh(&h, r.clone(), x(0));
            }
            "handleFillForFastRefresh" => {
                let fill = x(0);
                crate::refresh_entrypoints::handle_fill_for_fast_refresh(
                    if fill.is_nullish() { None } else { Some(&fill) },
                    || {
                        let f = r.get("queueFastPrivateRefresh")?;
                        let r = r.clone();
                        Ok(move |s: Option<Js>| f.call(r, vec![s.unwrap_or(Js::Undefined)]))
                    },
                    |v| v.get("symbol"),
                )?;
                return Ok(Js::Undefined);
            }
            "formatCreateOrder" => return crate::trading_rest::format_order(&h, r.clone(), x(0)),
            "listenOHLCV" => {
                return crate::runtime_lifecycle::listen_candles(&h, r.clone(), x(0), x(1));
            }
            "listenOrderBook" => {
                return crate::runtime_lifecycle::listen_book(&h, r.clone(), x(0), x(1));
            }
            _ => {}
        }
        let runtime = h.exchange_runtime();
        let r = r.clone();
        let (a0, a1, a2) = (x(0), x(1), x(2));
        let body: JsFuture<'static> = Box::pin(async move {
            match name {
                "validateAccount" => crate::runtime_lifecycle::validate(&h, r).await,
                "start" => crate::runtime_lifecycle::start(&h, r).await,
                "tick" => crate::runtime_lifecycle::tick(&h, r).await,
                "runFastPrivateRefresh" => crate::runtime_lifecycle::run_refresh(&h, r).await,
                "fetchBitrueTradeRows" => crate::collection_reads::trade_rows(&h, r, a0).await,
                "fetchOpenPositions" => crate::collection_reads::open_positions(&h, r).await,
                "fetchPositions" => crate::collection_reads::positions(&h, r, a0).await,
                "fetchBalanceAndPositions" => {
                    crate::collection_reads::balance_positions(&h, r).await
                }
                "parsePositions" => crate::runtime_lifecycle::parse_positions(&h, r, a0).await,
                "fetchOrders" => crate::collection_reads::orders(&h, r, a0).await,
                "fetchAndEmitOrders" => crate::collection_reads::emit_orders(&h, r).await,
                "fetchOrdersForSymbol" => crate::collection_reads::symbol_orders(&h, r, a0).await,
                "fetchHistoricalTradesInternal" => {
                    crate::collection_reads::history(&h, r, a0).await
                }
                "fetchHistoricalFills" => crate::collection_reads::fills(&h, r, a0).await,
                "fetchMarkets" => crate::trading_rest::markets(&h, r).await,
                "fetchOHLCV" => crate::trading_rest::candles(&h, r, a0).await,
                "placeOrder" => crate::trading_rest::place(&h, r, a0).await,
                "placeOrders" => crate::trading_rest::places(&h, r, a0).await,
                "placeOrderBatch" => crate::trading_rest::batch(&h, r, a0).await,
                "updateOrder" => crate::trading_rest::update(&h, r, a0).await,
                "setLeverage" => crate::trading_rest::leverage(&h, r, a0, a1, a2).await,
                "cancelOrders" => crate::trading_rest::cancel(&h, r, a0).await,
                "changeMarginType" => {
                    crate::refresh_entrypoints::change_margin_type(
                        a0,
                        a1,
                        |m| r.call_method("emitAndLogError", vec![Js::str(m)]),
                        |m| Ok(Thrown::error("Error", m)),
                    )
                    .await?;
                    Ok(Js::Undefined)
                }
                "fetchBalanceAndPositionsRaw" => account(&h, &r).await,
                "fetchBalance" => {
                    let data = await_js(
                        &h.exchange_runtime(),
                        r.call_method("fetchBalanceAndPositionsRaw", vec![])?,
                    )
                    .await?;
                    r.call_method("parseBalance", vec![data])
                }
                "fetchAndSyncSymbolMaxLeverage"
                | "fetchSymbolMaxLeverage"
                | "updateSymbolMetadata" => leverage(&h, &r, name, a0).await,
                "fetchPositionMetadata" => metadata(&h, &r, a0, a1).await,
                _ => Err(Thrown::error(
                    "Error",
                    &format!("Unbound Bitrue exchange member {name}"),
                )),
            }
        });
        Ok(Js::Promise(Promise::spawn(&runtime, body)))
    })
}

async fn account(h: &Rt, r: &Js) -> Result<Js, Thrown> {
    let xhr = r.get("xhr")?;
    let owner = r.clone();
    let result = crate::account_reads::fetch_balance_and_positions_raw(
        move |url| {
            let request = xhr
                .call_method("get", vec![Js::str(url)])
                .map_err(|e| e.to_exchange_error())?;
            Ok(async move {
                let response = await_js(&h.exchange_runtime(), request)
                    .await
                    .map_err(|e| e.to_exchange_error())?;
                let data = response.get("data").map_err(|e| e.to_exchange_error())?;
                if data.is_undefined() {
                    Ok(None)
                } else {
                    to_json(&data).map(Some).map_err(|e| e.to_exchange_error())
                }
            })
        },
        move |m| {
            owner
                .call_method("emitAndLogError", vec![m.map_or(Js::Undefined, from_json)])
                .map(|_| ())
                .map_err(|e| e.to_exchange_error())
        },
    )
    .await
    .map_err(exchange_error)?;
    let _ = h;
    Ok(result.as_ref().map_or(Js::Undefined, from_json))
}
async fn leverage(h: &Rt, r: &Js, name: &str, symbol: Js) -> Result<Js, Thrown> {
    let symbol = text(&symbol)?;
    let public = r.get("isPublicDataOnly")?.truthy();
    let xhr = r.get("xhr")?;
    let get = move |url: &'static str, config: serde_json::Value| {
        let p = xhr
            .call_method("get", vec![Js::str(url), from_json(&config)])
            .map_err(|e| e.to_exchange_error())?;
        Ok(async move {
            let v = await_js(&h.exchange_runtime(), p)
                .await
                .map_err(|e| e.to_exchange_error())?
                .get("data")
                .map_err(|e| e.to_exchange_error())?;
            if v.is_undefined() {
                Ok(None)
            } else {
                to_json(&v).map(Some).map_err(|e| e.to_exchange_error())
            }
        })
    };
    if name == "fetchSymbolMaxLeverage" {
        return Ok(Js::Number(
            crate::leverage_reads::fetch_symbol_max_leverage(
                h.shared.store.as_ref(),
                &symbol,
                public,
                get,
            )
            .await,
        ));
    }
    let value = crate::leverage_reads::fetch_and_sync_symbol_max_leverage(
        h.shared.store.as_ref(),
        &symbol,
        public,
        get,
    )
    .await;
    Ok(if name == "updateSymbolMetadata" {
        Js::Undefined
    } else {
        value.map_or(Js::Null, Js::Number)
    })
}
async fn metadata(h: &Rt, r: &Js, symbols: Js, mut supplied: Js) -> Result<Js, Thrown> {
    let symbols = super::host::items(&symbols)?;
    if !supplied.truthy() {
        let map = r.get("positionMetadataBySymbol")?;
        let mut missing = false;
        for symbol in &symbols {
            if !map.call_method("has", vec![symbol.clone()])?.truthy() {
                missing = true;
                break;
            }
        }
        if missing {
            let account = await_js(
                &h.exchange_runtime(),
                r.call_method("fetchBalanceAndPositionsRaw", vec![])?,
            )
            .await?;
            supplied = r.call_method("getAccountPositionSnapshots", vec![account])?;
        }
    }
    if supplied.truthy() {
        let supplied = to_json(&supplied)?;
        let rows = supplied
            .as_array()
            .ok_or_else(|| Thrown::type_error("position snapshots must be an array"))?;
        let snapshots = rows
            .iter()
            .map(|row| {
                Ok(crate::account_helpers::AccountPositionSnapshot {
                    symbol: row["symbol"]
                        .as_str()
                        .ok_or_else(|| Thrown::type_error("position symbol must be a string"))?,
                    side: serde_json::from_value(row["side"].clone())
                        .map_err(|e| Thrown::type_error(&e.to_string()))?,
                    leverage_level: &row["leverageLevel"],
                    position_type: &row["positionType"],
                })
            })
            .collect::<Result<Vec<_>, Thrown>>()?;
        let names = symbols.iter().map(text).collect::<Result<Vec<_>, _>>()?;
        let owner = r.clone();
        // Reuse the existing typed metadata computation for a supplied snapshot.
        // The public JS Map, rather than this numeric bridge, owns retained aliases.
        crate::position_metadata::fetch_position_metadata(
            &h.shared.extra.metadata,
            &names,
            Some(&snapshots),
            move || {
                let p = owner
                    .call_method("fetchBalanceAndPositionsRaw", vec![])
                    .map_err(|e| e.to_exchange_error())?;
                Ok(async move {
                    let v = await_js(&h.exchange_runtime(), p)
                        .await
                        .map_err(|e| e.to_exchange_error())?;
                    if v.is_undefined() {
                        Ok(None)
                    } else {
                        to_json(&v).map(Some).map_err(|e| e.to_exchange_error())
                    }
                })
            },
            || false,
        )
        .await
        .map_err(exchange_error)?;
        let map = Js::new_map();
        for (key, row) in h.shared.extra.metadata.borrow().iter() {
            let v = row.borrow();
            let value = Js::object(vec![
                ("symbol", Js::str(&v.symbol)),
                ("side", from_typed(&v.side)?),
                ("leverage", Js::Number(v.leverage)),
                ("positionMode", from_typed(&v.position_mode)?),
                ("marginType", from_typed(&v.margin_type)?),
            ]);
            map.call_method("set", vec![Js::str(key), value])?;
        }
        r.set("positionMetadataBySymbol", map)?;
    }
    if r.get("isDisposed")?.truthy() {
        return Ok(Js::array(vec![]));
    }
    let map = r.get("positionMetadataBySymbol")?;
    let mut out = vec![];
    for symbol in symbols {
        let value = map.call_method("get", vec![symbol])?;
        if value.truthy() {
            out.push(value)
        }
    }
    Ok(Js::array(out))
}
