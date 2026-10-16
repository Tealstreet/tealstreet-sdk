use crate::{
    class_construction::{self, ClassOps},
    futures_public_class::ConstructionOps,
    spot_bodies::{self, SpotBodyOps},
    spot_native_accounts as accounts, spot_native_order_reads as reads,
    spot_native_policy as policy,
    spot_private_class::{PrivateClassOps, call},
};
use std::{cell::RefCell, rc::Rc};

pub struct SpotExchangeOps<V, E> {
    pub language: Rc<PrivateClassOps<V, E>>,
    pub construction: Rc<ConstructionOps<V, E>>,
    /// Pure acquisition of standard language capabilities over the same receiver.
    pub body: Rc<dyn Fn(&V) -> Rc<SpotBodyOps<V, E>>>,
    pub regexp: Rc<dyn Fn(&str) -> Result<V, E>>,
}
fn arity(name: &str) -> Option<(usize, bool)> {
    Some(match name {
        "dispose" => (0, false),
        "getUsdPrice" | "mapOrder" => (1, false),
        "listenOHLCV" | "listenOrderBook" => (2, false),
        "getAccount"
        | "validateAccount"
        | "start"
        | "fetchBalance"
        | "tick"
        | "fetchActiveOrderSymbols"
        | "cancelAllOrders"
        | "fetchMarkets"
        | "fetchTickers" => (0, true),
        "fetchPositions"
        | "fetchSymbolMaxLeverage"
        | "fetchOrders"
        | "placeOrder"
        | "updateOrder"
        | "cancelOrders"
        | "cancelSymbolOrders"
        | "fetchHistoricalFills"
        | "fetchOHLCV" => (1, true),
        "setLeverage" => (3, true),
        "changeMarginType" | "changePositionMode" => (2, true),
        _ => return None,
    })
}
fn native_member<V: Clone + 'static, E: 'static>(
    ops: Rc<SpotExchangeOps<V, E>>,
    home: V,
    receiver: V,
    name: &'static str,
) -> Result<V, E> {
    let language = ops.language.clone();
    let Some((length, asynchronous)) = arity(name) else {
        return Err((language.error)("unknown KucoinSpotExchange field"));
    };
    if !asynchronous {
        let owner = ops.clone();
        return Ok((language.arrow)(
            name,
            length,
            Rc::new(move |args| {
                let h = (owner.language.frame)(&receiver);
                let arg = |index| args.get(index).cloned().unwrap_or_else(|| (h.undefined)());
                match name {
                    "dispose" => {
                        let member = (owner.language.super_member)(&home, &receiver, "dispose")?;
                        (h.invoke)(member, vec![])?;
                        call(&h, "this.publicWebsocket.dispose", || Ok(vec![]))?;
                        call(&h, "this.privateWebsocket.dispose", || Ok(vec![]))?;
                        Ok((h.undefined)())
                    }
                    "getUsdPrice" => policy::get_usd_price(&owner.language, &h, arg(0)),
                    "mapOrder" => reads::map_order(&owner.language, &h, arg(0)),
                    "listenOHLCV" => reads::listen_ohlcv(&h, arg(0), arg(1)),
                    "listenOrderBook" => reads::listen_order_book(&h, arg(0), arg(1)),
                    _ => Err((owner.language.error)(
                        "unknown synchronous KucoinSpotExchange field",
                    )),
                }
            }),
        ));
    }
    Ok((language.async_arrow)(
        name,
        length,
        Rc::new(move |args| {
            let owner = ops.clone();
            let receiver = receiver.clone();
            Box::pin(async move {
                let h = (owner.language.frame)(&receiver);
                let arg = |index| args.get(index).cloned().unwrap_or_else(|| (h.undefined)());
                match name {
                    "getAccount" => accounts::get_account(h).await,
                    "validateAccount" => accounts::validate_account(h).await,
                    "start" => spot_bodies::start((owner.body)(&receiver)).await,
                    "fetchBalance" => spot_bodies::fetch_balance((owner.body)(&receiver)).await,
                    "fetchPositions" => policy::fetch_positions(
                        owner.language.clone(),
                        &owner.construction,
                        receiver.clone(),
                        arg(0),
                    ),
                    "fetchSymbolMaxLeverage" => Ok(policy::fetch_symbol_max_leverage(&h)),
                    "setLeverage" => policy::set_leverage(&h, arg(0)),
                    "changeMarginType" => Ok(policy::change_margin_type(&h)),
                    "changePositionMode" => Ok(policy::change_position_mode(&h)),
                    "tick" => spot_bodies::tick((owner.body)(&receiver)).await,
                    "fetchOrders" => {
                        spot_bodies::fetch_orders((owner.body)(&receiver), arg(0)).await
                    }
                    "fetchActiveOrderSymbols" => reads::fetch_active_order_symbols(h).await,
                    "placeOrder" => spot_bodies::place_order((owner.body)(&receiver), arg(0)).await,
                    "updateOrder" => {
                        spot_bodies::update_order((owner.body)(&receiver), arg(0)).await
                    }
                    "cancelOrders" => {
                        reads::cancel_orders(
                            owner.language.clone(),
                            (owner.body)(&receiver),
                            arg(0),
                        )
                        .await
                    }
                    "cancelSymbolOrders" => {
                        reads::cancel_symbol_orders(
                            owner.language.clone(),
                            (owner.body)(&receiver),
                            arg(0),
                        )
                        .await
                    }
                    "cancelAllOrders" => reads::cancel_all_orders((owner.body)(&receiver)).await,
                    "fetchHistoricalFills" => {
                        spot_bodies::fetch_historical_fills((owner.body)(&receiver), arg(0)).await
                    }
                    "fetchMarkets" => reads::fetch_markets(owner.language.clone(), h).await,
                    "fetchTickers" => reads::fetch_tickers(owner.language.clone(), h).await,
                    "fetchOHLCV" => spot_bodies::fetch_ohlcv((owner.body)(&receiver), arg(0)).await,
                    _ => Err((owner.language.error)(
                        "unknown asynchronous KucoinSpotExchange field",
                    )),
                }
            })
        }),
    ))
}
pub fn define_spot_exchange_class<V: Clone + 'static, E: 'static>(
    ops: Rc<SpotExchangeOps<V, E>>,
) -> Result<V, E> {
    let home = Rc::new(RefCell::new(None));
    let capture_home = home.clone();
    let members = ops.clone();
    let receiver_ops = ops.language.clone();
    let h = Rc::new(ClassOps {
        value: ops.language.value.clone(),
        null: ops.language.null.clone(),
        boolean: ops.language.boolean.clone(),
        regexp: ops.regexp.clone(),
        define: ops.language.define.clone(),
        member: Rc::new(move |receiver, name| {
            let h = (receiver_ops.frame)(receiver);
            (h.capture)(&format!("this.{name}"))
        }),
        superclass: ops.construction.superclass.clone(),
        super_construct: ops.construction.super_construct.clone(),
        construct: ops.construction.construct.clone(),
        function: ops.language.arrow.clone(),
        native_member: Rc::new(move |receiver, _, name, _| {
            let home = capture_home.borrow().as_ref().cloned().ok_or_else(|| {
                (members.language.error)("KucoinSpotExchange class not initialized")
            })?;
            native_member(members.clone(), home, receiver.clone(), name)
        }),
        prototype_member: Rc::new(|_, _, _, _, _, _| {
            unreachable!("Spot exchange has only own arrow fields")
        }),
        derive: ops.construction.derive.clone(),
    });
    let class = class_construction::define_spot_exchange(h)?;
    *home.borrow_mut() = Some(class.clone());
    Ok(class)
}
