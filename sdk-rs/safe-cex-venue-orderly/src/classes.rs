//! Exchange class factories over explicit source superclass, import and member bindings.
use crate::{
    account_order_reads::AccountReadHost,
    ws_controls::{Result, Value},
};
use std::{future::Future, pin::Pin, rc::Rc};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Orderly,
    KodiakFI,
    Mode,
    SalsaDEX,
    TealstreetDEX,
    What,
    Woofi,
}
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Orderly => "OrderlyExchange",
            Self::KodiakFI => "KodiakFIExchange",
            Self::Mode => "MODEExchange",
            Self::SalsaDEX => "SalsaDEXExchange",
            Self::TealstreetDEX => "TealstreetDEXExchange",
            Self::What => "WhatExchange",
            Self::Woofi => "WoofiExchange",
        }
    }
    pub fn tag(self) -> Option<&'static str> {
        match self {
            Self::Orderly => None,
            Self::KodiakFI => Some("KODIAKFI"),
            Self::Mode => Some("MODE"),
            Self::SalsaDEX => Some("SALSADEX"),
            Self::TealstreetDEX => Some("Tealstreet DEX"),
            Self::What => Some("WHAT"),
            Self::Woofi => Some("WOOFI"),
        }
    }
}
pub type BodyFuture<H> = Pin<Box<dyn Future<Output = Result<H, Value<H>>>>>;
pub enum Return<H: AccountReadHost> {
    Value(Value<H>),
    Future(BodyFuture<H>),
}
pub type NativeMember<H> = Rc<dyn Fn(Value<H>, Vec<Value<H>>) -> Result<H, Return<H>>>;
pub type BoundMember<H> = Rc<dyn Fn(Vec<Value<H>>) -> Result<H, Return<H>>>;
#[derive(Clone, Copy)]
pub enum Initializer {
    Undefined,
    Text(&'static str),
    Number(f64),
    Null,
    Map,
    Rule,
    Imported(&'static str),
    Arrow(&'static str),
    Prototype(&'static str),
}
pub struct Definition {
    pub name: &'static str,
    pub initializer: Initializer,
}
pub trait ClassHost: AccountReadHost + 'static {
    fn spread_object(&self, value: Value<Self>) -> Result<Self, Value<Self>>;
    fn starts_with(&self, value: Value<Self>, prefix: &str) -> Result<Self, bool>;
    fn super_exchange(
        &self,
        new_target: Kind,
        name: Value<Self>,
        options: Value<Self>,
        store: Value<Self>,
    ) -> Result<Self, Value<Self>>;
    fn define(&self, receiver: Value<Self>, name: &str, value: Value<Self>) -> Result<Self, ()>;
    fn fresh_map(&self) -> Result<Self, Value<Self>>;
    fn client_rule(&self, name: &str, pattern: &str) -> Result<Self, Value<Self>>;
    fn imported(&self, name: &'static str) -> Value<Self>;
    // Provision every original member ABI without observable effects.
    fn member(&self, name: &'static str, target: &'static str) -> NativeMember<Self>;
    fn arrow(&self, name: &'static str, body: BoundMember<Self>) -> Value<Self>;
    // Register once; later constructions preserve the existing prototype member.
    fn prototype(&self, name: &'static str, body: NativeMember<Self>);
    fn create_api(&self, options: Value<Self>, receiver: Value<Self>) -> Result<Self, Value<Self>>;
    fn rate_limit(&self, client: Value<Self>, max_rps: f64) -> Result<Self, Value<Self>>;
    fn construct_socket(&self, private: bool, parent: Value<Self>) -> Result<Self, Value<Self>>;
}
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn get<H: ClassHost>(h: &H, v: &Value<H>, key: &str) -> Result<H, Value<H>> {
    h.get(v, &units(key))
}
fn text<H: ClassHost>(h: &H, s: &str) -> Value<H> {
    h.text(&units(s))
}
fn prepare<H: ClassHost>(h: &H) {
    for d in definitions() {
        if let Initializer::Prototype(target) = d.initializer {
            h.prototype(d.name, h.member(d.name, target));
        }
    }
}
pub fn initialize<H: ClassHost>(h: &H, receiver: Value<H>) -> Result<H, ()> {
    for d in definitions() {
        let value = match d.initializer {
            Initializer::Undefined => h.undefined(),
            Initializer::Text(s) => text(h, s),
            Initializer::Number(n) => h.number(n),
            Initializer::Null => h.null(),
            Initializer::Map => h.fresh_map()?,
            Initializer::Rule => h.client_rule("Orderly", r"^(?!-).{1,36}$")?,
            Initializer::Imported(s) => h.imported(s),
            Initializer::Arrow(target) => {
                let f = h.member(d.name, target);
                let captured = receiver.clone();
                h.arrow(d.name, Rc::new(move |args| f(captured.clone(), args)))
            }
            Initializer::Prototype(_) => continue,
        };
        h.define(receiver.clone(), d.name, value)?;
    }
    Ok(())
}
fn construct_with<H: ClassHost>(
    h: &H,
    kind: Kind,
    name: Value<H>,
    options: Value<H>,
    store: Value<H>,
) -> Result<H, Value<H>> {
    let normalized = h.spread_object(options)?;
    for key in ["key", "secret"] {
        if h.truthy(&*get(h, &normalized, key)?)
            && !h.starts_with(get(h, &normalized, key)?, "ed25519:")?
        {
            let value =
                h.text(&[units("ed25519:"), h.string(&*get(h, &normalized, key)?)?].concat());
            h.set(&normalized, &units(key), value)?;
        }
    }
    let receiver = h.super_exchange(kind, name, normalized.clone(), store)?;
    initialize(h, receiver.clone())?;
    let client = h.create_api(normalized, receiver.clone())?;
    let client = h.rate_limit(client, 10.0)?;
    h.set(&receiver, &units("xhr"), client)?;
    let public = h.construct_socket(false, receiver.clone())?;
    h.set(&receiver, &units("publicWebsocket"), public)?;
    let private = h.construct_socket(true, receiver.clone())?;
    h.set(&receiver, &units("privateWebsocket"), private)?;
    Ok(receiver)
}
pub fn construct<H: ClassHost>(h: &H, kind: Kind, args: Vec<Value<H>>) -> Result<H, Value<H>> {
    prepare(h);
    let name = args.first().cloned().unwrap_or_else(|| h.undefined());
    let options = args.get(1).cloned().unwrap_or_else(|| h.undefined());
    let store = args.get(2).cloned().unwrap_or_else(|| h.undefined());
    let receiver = construct_with(h, kind, name, options, store)?;
    if let Some(tag) = kind.tag() {
        h.define(receiver.clone(), "orderlyExchangeName", text(h, tag))?;
    }
    Ok(receiver)
}
pub fn definitions() -> &'static [Definition] {
    &[
        Definition {
            name: "clientOrderIdRule",
            initializer: Initializer::Rule,
        },
        Definition {
            name: "xhr",
            initializer: Initializer::Undefined,
        },
        Definition {
            name: "publicWebsocket",
            initializer: Initializer::Undefined,
        },
        Definition {
            name: "privateWebsocket",
            initializer: Initializer::Undefined,
        },
        Definition {
            name: "grafanaEndpointsPeriodic",
            initializer: Initializer::Imported("GRAFANA_ENDPOINTS_PERIODIC"),
        },
        Definition {
            name: "grafanaEndpointsEvery",
            initializer: Initializer::Imported("GRAFANA_ENDPOINTS_EVERY"),
        },
        Definition {
            name: "orderlyExchangeName",
            initializer: Initializer::Text("orderly"),
        },
        Definition {
            name: "accountLeverage",
            initializer: Initializer::Number(1.0),
        },
        Definition {
            name: "_accountPositionMode",
            initializer: Initializer::Null,
        },
        Definition {
            name: "marginTypeBySymbol",
            initializer: Initializer::Map,
        },
        Definition {
            name: "dispose",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/runtime_actions.rs::dispose",
            ),
        },
        Definition {
            name: "getPositionMode",
            initializer: Initializer::Prototype(
                "safe-cex-venue-orderly/src/account_order_reads.rs::position_mode",
            ),
        },
        Definition {
            name: "getAccount",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/account_order_reads.rs::account",
            ),
        },
        Definition {
            name: "validateAccount",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/account_order_reads.rs::validate",
            ),
        },
        Definition {
            name: "start",
            initializer: Initializer::Arrow("safe-cex-venue-orderly/src/runtime_actions.rs::start"),
        },
        Definition {
            name: "tick",
            initializer: Initializer::Arrow("safe-cex-venue-orderly/src/runtime_actions.rs::tick"),
        },
        Definition {
            name: "fetchAndUpdateAccountLeverage",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/account_order_reads.rs::leverage",
            ),
        },
        Definition {
            name: "fetchAndUpdateMarginTypes",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/account_order_reads.rs::margin_types",
            ),
        },
        Definition {
            name: "fetchMarkets",
            initializer: Initializer::Arrow("safe-cex-venue-orderly/src/data_mappers.rs::markets"),
        },
        Definition {
            name: "fetchTickers",
            initializer: Initializer::Arrow("safe-cex-venue-orderly/src/data_mappers.rs::tickers"),
        },
        Definition {
            name: "fetchPositions",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/data_mappers.rs::positions",
            ),
        },
        Definition {
            name: "mapMarginType",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/margin.rs::map_margin_type",
            ),
        },
        Definition {
            name: "toOrderlyMarginMode",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/margin.rs::to_orderly_margin_mode",
            ),
        },
        Definition {
            name: "getCurrentMarginType",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/margin.rs::get_current_margin_type",
            ),
        },
        Definition {
            name: "createOrderlyPositionId",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/margin.rs::create_orderly_position_id",
            ),
        },
        Definition {
            name: "mapPosition",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/data_mappers.rs::map_position",
            ),
        },
        Definition {
            name: "fetchOrders",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/account_order_reads.rs::orders",
            ),
        },
        Definition {
            name: "fetchOHLCV",
            initializer: Initializer::Arrow("safe-cex-venue-orderly/src/data_mappers.rs::candles"),
        },
        Definition {
            name: "listenOHLCV",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/data_mappers.rs::listen_candles",
            ),
        },
        Definition {
            name: "listenOrderBook",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/data_mappers.rs::listen_book",
            ),
        },
        Definition {
            name: "updateOrder",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/runtime_actions.rs::update_order",
            ),
        },
        Definition {
            name: "updateAlgoOrder",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/trading_actions.rs::update_algo",
            ),
        },
        Definition {
            name: "fetchLimitOrders",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/account_order_reads.rs::limit_orders",
            ),
        },
        Definition {
            name: "mapLimitOrder",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/data_mappers.rs::map_limit",
            ),
        },
        Definition {
            name: "fetchAlgoOrders",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/account_order_reads.rs::algo_orders",
            ),
        },
        Definition {
            name: "mapAlgoOrder",
            initializer: Initializer::Arrow("safe-cex-venue-orderly/src/data_mappers.rs::map_algo"),
        },
        Definition {
            name: "cancelOrders",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/trading_actions.rs::cancel_orders",
            ),
        },
        Definition {
            name: "cancelAlgoOrder",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/trading_actions.rs::cancel_algo",
            ),
        },
        Definition {
            name: "getPositionSideForOrder",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/utils.rs::get_position_side_for_order",
            ),
        },
        Definition {
            name: "placeOrder",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/runtime_actions.rs::place_order",
            ),
        },
        Definition {
            name: "placeAlgoOrder",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/runtime_actions.rs::place_algo",
            ),
        },
        Definition {
            name: "resolveMarginTypeForOrder",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/margin.rs::resolve_margin_type_for_order",
            ),
        },
        Definition {
            name: "placePositionalAlgoOrder",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/runtime_actions.rs::place_positional",
            ),
        },
        Definition {
            name: "cancelAllOrders",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/trading_actions.rs::cancel_all",
            ),
        },
        Definition {
            name: "cancelSymbolOrders",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/trading_actions.rs::cancel_symbol",
            ),
        },
        Definition {
            name: "setLeverage",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/trading_actions.rs::set_leverage",
            ),
        },
        Definition {
            name: "setAllLeverage",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/trading_actions.rs::set_all_leverage",
            ),
        },
        Definition {
            name: "changeMarginType",
            initializer: Initializer::Arrow(
                "safe-cex-venue-orderly/src/trading_actions.rs::change_margin",
            ),
        },
        Definition {
            name: "isAlgoOrder",
            initializer: Initializer::Arrow("safe-cex-venue-orderly/src/utils.rs::is_algo_order"),
        },
    ]
}
