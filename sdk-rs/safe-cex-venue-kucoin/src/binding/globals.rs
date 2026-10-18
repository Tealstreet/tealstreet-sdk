//! KuCoin module scope: protocol tables, the imported helpers the source
//! reads, and the BaseExchange/BaseWebSocket/createAPI construction globals.
use super::host::{W, shared, text};
use crate::{spot_types, types, ws_helpers};
use safe_cex_exchange::js_runtime::{
    base_exchange, client,
    value::{Js, Thrown, arg, from_json, from_typed, to_json, to_typed, utf16},
};
use safe_cex_types::{ExchangeName, OrderBook};
use std::rc::Rc;

fn table(rows: &[(&str, &str)]) -> Js {
    Js::object(rows.iter().map(|(k, v)| (*k, Js::str(v))).collect())
}

fn list(rows: &[&str]) -> Js {
    Js::array(rows.iter().map(|s| Js::str(s)).collect())
}

fn enum_table<T: serde::Serialize>(rows: &[(&str, T)]) -> Js {
    Js::object(
        rows.iter()
            .map(|(k, v)| {
                let text = serde_json::to_value(v)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default();
                (*k, Js::str(&text))
            })
            .collect(),
    )
}

fn number(v: &Js) -> Result<f64, Thrown> {
    v.to_number()
}

/// `^[A-Za-z0-9_-]{1,40}$`, the only pattern either KuCoin class declares.
fn kucoin_client_order_id(id: &str) -> bool {
    (1..=40).contains(&id.chars().count())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

const CLIENT_ORDER_ID_SOURCE: &str = "^[A-Za-z0-9_-]{1,40}$";

pub(crate) fn client_order_id_rule(
    spot: bool,
) -> safe_cex_exchange::client_order_id::ClientOrderIdRule {
    safe_cex_exchange::client_order_id::pattern_client_order_id_rule(
        if spot { "KuCoin Spot" } else { "KuCoin" }.into(),
        format!("/{CLIENT_ORDER_ID_SOURCE}/"),
        Rc::new(kucoin_client_order_id),
        Default::default(),
    )
}

/// `/source/` on the shared RegExp.
pub(crate) fn regexp(source: &str) -> Result<Js, Thrown> {
    safe_cex_exchange::js_runtime::regexp::regexp(&utf16(source), &[])
}

/// Apply a typed OrderBook helper to a live JS book, writing both sides back.
fn with_book(book: &Js, f: impl FnOnce(&mut OrderBook) -> f64) -> Result<f64, Thrown> {
    let mut typed: OrderBook = to_typed(book)?;
    let out = f(&mut typed);
    book.set("bids", from_typed(&typed.bids)?)?;
    book.set("asks", from_typed(&typed.asks)?)?;
    Ok(out)
}

/// The JS BaseExchange instance; its base `dispose` is stashed for `super.dispose()`.
fn base_exchange(weak: &W, a: Vec<Js>) -> Result<Js, Thrown> {
    let scope = shared(weak)?;
    let parts = scope
        .extra
        .parts
        .borrow_mut()
        .take()
        .ok_or_else(|| Thrown::error("Error", "BaseExchange constructed twice"))?;
    let (this, base) = base_exchange::create(
        &scope.sockets,
        parts,
        arg(&a, 0),
        arg(&a, 1),
        arg(&a, 2),
        |this| {
            // kucoinspot.exchange.ts: isSpot = true; Futures inherits false.
            if scope.extra.api.spot {
                this.define("isSpot", Js::Bool(true))?;
            }
            Ok(())
        },
    )?;
    let _ = scope.base.set(base);
    scope.extra.supers.borrow_mut().push((
        this.clone(),
        Js::object(vec![("dispose", this.get("dispose")?)]),
    ));
    Ok(this)
}

fn exchange_name(text: &str) -> Result<ExchangeName, Thrown> {
    serde_json::from_value(serde_json::Value::String(text.into()))
        .map_err(|e| Thrown::type_error(&e.to_string()))
}

fn json_arg(v: &Js) -> Result<Option<serde_json::Value>, Thrown> {
    Ok(if v.is_undefined() {
        None
    } else {
        Some(to_json(v)?)
    })
}

pub(crate) fn install(
    g: &Js,
    weak: W,
    runtime: &Rc<dyn safe_cex_exchange::websocket::WsRuntime>,
) -> Result<(), Thrown> {
    let def = |k: &str, v: Js| g.define(k, v);
    // kucoin.types.ts / kucoinspot.types.ts
    def("ENDPOINTS", table(types::ENDPOINTS))?;
    def(
        "GRAFANA_ENDPOINTS_EVERY",
        list(types::GRAFANA_ENDPOINTS_EVERY),
    )?;
    def(
        "GRAFANA_ENDPOINTS_PERIODIC",
        list(types::GRAFANA_ENDPOINTS_PERIODIC),
    )?;
    def("INTERVAL", table(types::INTERVAL))?;
    def("WS_INTERVAL", table(types::WS_INTERVAL))?;
    def("ORDER_STATUS", enum_table(types::ORDER_STATUS))?;
    def("ORDER_TYPE", enum_table(types::ORDER_TYPE))?;
    def("TIME_IN_FORCE", enum_table(types::TIME_IN_FORCE))?;
    def(
        "KUCOIN_SPOT_ENDPOINTS",
        table(spot_types::KUCOIN_SPOT_ENDPOINTS),
    )?;
    def(
        "KUCOIN_SPOT_GRAFANA_ENDPOINTS_EVERY",
        list(spot_types::KUCOIN_SPOT_GRAFANA_ENDPOINTS_EVERY),
    )?;
    def(
        "KUCOIN_SPOT_GRAFANA_ENDPOINTS_PERIODIC",
        list(spot_types::KUCOIN_SPOT_GRAFANA_ENDPOINTS_PERIODIC),
    )?;
    def(
        "KUCOIN_SPOT_INTERVAL",
        table(spot_types::KUCOIN_SPOT_INTERVAL),
    )?;
    def(
        "KUCOIN_SPOT_ORDER_STATUS",
        enum_table(spot_types::KUCOIN_SPOT_ORDER_STATUS),
    )?;
    def(
        "KUCOIN_SPOT_ORDER_TYPE",
        enum_table(spot_types::KUCOIN_SPOT_ORDER_TYPE),
    )?;
    def(
        "KUCOIN_SPOT_TIME_IN_FORCE",
        enum_table(spot_types::KUCOIN_SPOT_TIME_IN_FORCE),
    )?;
    // kucoin.ws-public.ts module constants
    def("SNAPSHOT_CHANNEL", Js::str("/contractMarket/snapshot"))?;
    def(
        "ACTIVE_DETAIL_TICKER_CHANNELS",
        list(&["/contractMarket/tickerV2", "/contract/instrument"]),
    )?;
    def(
        "ENABLE_RECONNECTING_WEBSOCKET",
        Js::Bool(runtime.enable_reconnecting()),
    )?;

    // Imported utils.
    def(
        "divide",
        Js::function("divide", |_, a| {
            Ok(Js::Number(safe_cex_num::divide(
                number(&arg(&a, 0))?,
                number(&arg(&a, 1))?,
                None,
            )))
        }),
    )?;
    def(
        "toBinance",
        Js::function("toBinance", |_, a| {
            Ok(Js::str(&safe_cex_types::symbol_converter::to_binance(
                &text(&arg(&a, 0))?,
                &exchange_name(&text(&arg(&a, 1))?)?,
            )))
        }),
    )?;
    def(
        "jsonParse",
        Js::function("jsonParse", |_, a| {
            let text = text(&arg(&a, 0))?;
            Ok(serde_json::from_str::<serde_json::Value>(&text)
                .map(|v| from_json(&v))
                .unwrap_or(Js::Null))
        }),
    )?;
    def(
        "parseNumber",
        Js::function("parseNumber", |_, a| {
            let value = json_arg(&arg(&a, 0))?;
            Ok(ws_helpers::parse_number(value.as_ref())
                .map_err(|e| Thrown::type_error(&e))?
                .map_or(Js::Undefined, Js::Number))
        }),
    )?;
    for (name, f) in [
        (
            "normalizeTimestamp",
            ws_helpers::normalize_timestamp
                as for<'v> fn(Option<&'v serde_json::Value>, f64) -> Result<f64, String>,
        ),
        ("normalizeMillis", ws_helpers::normalize_millis),
    ] {
        let clock = runtime.clone();
        def(
            name,
            Js::function(name, move |_, a| {
                let value = json_arg(&arg(&a, 0))?;
                Ok(Js::Number(
                    f(value.as_ref(), clock.now_ms()).map_err(|e| Thrown::type_error(&e))?,
                ))
            }),
        )?;
    }
    def(
        "sortOrderBook",
        Js::function("sortOrderBook", |_, a| {
            with_book(&arg(&a, 0), |b| {
                safe_cex_store::orderbook::sort_order_book(b);
                0.0
            })?;
            Ok(Js::Undefined)
        }),
    )?;
    def(
        "updateOrderBookByPrice",
        Js::function("updateOrderBookByPrice", |_, a| {
            let bids = text(&arg(&a, 1))? == "bids";
            let (price, amount) = (number(&arg(&a, 2))?, number(&arg(&a, 3))?);
            let upsert = arg(&a, 4).is_undefined() || arg(&a, 4).truthy();
            let index = with_book(&arg(&a, 0), |b| {
                safe_cex_store::update_order_book::update_order_book_by_price(
                    b, bids, price, amount, upsert,
                ) as f64
            })?;
            Ok(Js::Number(index))
        }),
    )?;
    let snapshots = weak.clone();
    def(
        "createKucoinSnapshotWebsockets",
        Js::function("createKucoinSnapshotWebsockets", move |_, a| {
            let parent = arg(&a, 0);
            let count = number(&arg(&a, 1))?;
            let class = shared(&snapshots)?.globals.get("KucoinPublicWebsocket")?;
            crate::socket_setup::create_snapshot_websockets(
                &parent,
                count,
                |length, build| {
                    let mut out = vec![];
                    let length = if length.is_finite() && length > 0.0 {
                        length as usize
                    } else {
                        0
                    };
                    for i in 0..length {
                        out.push(build(i)?);
                    }
                    Ok(Js::array(out))
                },
                |parent, role, index, shards| {
                    class.construct(
                        vec![
                            parent.clone(),
                            Js::object(vec![
                                ("role", Js::str(role)),
                                ("shardIndex", Js::Number(index as f64)),
                                ("shardCount", Js::Number(shards)),
                            ]),
                        ],
                        None,
                    )
                },
            )
        }),
    )?;

    // Construction-time collaborators.
    let w = weak.clone();
    let base = Js::constructor("BaseExchange", None, move |_, a| base_exchange(&w, a));
    base.define("prototype", Js::object(vec![]))?;
    def("BaseExchange", base)?;
    for name in ["createAPI", "createKucoinSpotAPI"] {
        let w = weak.clone();
        def(
            name,
            Js::function(name, move |_, a| {
                let host = shared(&w)?
                    .extra
                    .api
                    .with_options(base_exchange::exchange_options(&arg(&a, 0))?);
                Ok(client::create(host, Js::object(vec![])))
            }),
        )?;
    }
    def(
        "rateLimit",
        Js::function("rateLimit", |_, a| {
            let xhr = arg(&a, 0);
            xhr.call_method("__configureRateLimit", vec![arg(&a, 1)])?;
            Ok(xhr)
        }),
    )?;
    def(
        "patternClientOrderIdRule",
        Js::function("patternClientOrderIdRule", |_, a| {
            let pattern = arg(&a, 1);
            let source = text(&pattern.get("source")?)?;
            Ok(Js::object(vec![
                ("label", arg(&a, 0)),
                ("requirement", Js::str(&format!("match /{source}/"))),
                (
                    "isValid",
                    Js::function("isValid", move |_, v| pattern.call_method("test", v)),
                ),
            ]))
        }),
    )?;
    Ok(())
}
