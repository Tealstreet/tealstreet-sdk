use crate::{
    futures_public_class::ConstructionOps,
    public_streams::StreamOps,
    spot_private_class::{PrivateClassOps, fallback, optional},
};
use std::rc::Rc;

fn suffix<V, E>(ops: &PrivateClassOps<V, E>, value: &V, suffix: &str) -> Result<V, E> {
    let mut units = (ops.template)(value)?;
    units.extend(suffix.encode_utf16());
    Ok((ops.string_value)(units))
}
pub fn get_usd_price<V: Clone, E>(
    ops: &PrivateClassOps<V, E>,
    h: &StreamOps<V, E>,
    currency: V,
) -> Result<V, E> {
    for name in ["USD", "USDT", "USDC"] {
        if (h.equal)(&currency, &(h.literal)(name)) {
            return Ok((h.number)(1.));
        }
    }
    let read = |quote| {
        let table = (h.capture)("this.store.tickersBySymbol")?;
        let key = suffix(ops, &currency, quote)?;
        (h.intrinsic)("get-index", vec![table, key])
    };
    let ticker = fallback(h, read("-USDT")?, || {
        fallback(h, read("-USDC")?, || read("-USD"))
    })?;
    fallback(h, optional(h, &ticker, "last")?, || Ok((h.number)(0.)))
}
pub fn fetch_positions<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    construction: &ConstructionOps<V, E>,
    receiver: V,
    symbols: V,
) -> Result<V, E> {
    let h = (ops.frame)(&receiver);
    let selection = if (h.truthy)(&symbols) {
        let ctor = (h.capture)("Set")?;
        (construction.construct)(ctor, vec![symbols])?
    } else {
        (ops.null)()
    };
    let filter = (h.capture)("this.store.markets.filter")?;
    let value = h.clone();
    let booleans = ops.clone();
    let predicate = (ops.arrow)(
        "",
        1,
        Rc::new(move |args| {
            let market = args.first().cloned().unwrap_or_else(|| (value.undefined)());
            let active = (value.get)(&market, "active")?;
            if !(value.truthy)(&active) {
                return Ok(active);
            }
            if !(value.truthy)(&selection) {
                return Ok((booleans.boolean)(true));
            }
            let has = (value.get)(&selection, "has")?;
            let symbol = (value.get)(&market, "symbol")?;
            (value.intrinsic)(
                "call-with-this",
                vec![has, selection.clone(), (value.array)(vec![symbol])],
            )
        }),
    );
    let filtered = (h.invoke)(filter, vec![predicate])?;
    let map = (h.get)(&filtered, "map")?;
    let value = h.clone();
    let metadata = ops.clone();
    let mapper = (ops.arrow)(
        "",
        1,
        Rc::new(move |args| {
            let market = args.first().cloned().unwrap_or_else(|| (value.undefined)());
            let id = suffix(&metadata, &(value.get)(&market, "symbol")?, ":long")?;
            Ok((value.object)(vec![
                ("id", id),
                ("symbol", (value.get)(&market, "symbol")?),
                ("side", (value.capture)("PositionSide.Long")?),
                ("entryPrice", (value.number)(0.)),
                ("notional", (value.number)(0.)),
                ("leverage", (value.number)(1.)),
                ("unrealizedPnl", (value.number)(0.)),
                ("contracts", (value.number)(0.)),
                ("liquidationPrice", (value.number)(0.)),
                ("marginType", (value.capture)("MarginType.Isolated")?),
                ("positionMode", (value.capture)("PositionMode.OneWay")?),
                (
                    "info",
                    (value.object)(vec![("spot", (metadata.boolean)(true))]),
                ),
            ]))
        }),
    );
    (h.intrinsic)(
        "call-with-this",
        vec![map, filtered, (h.array)(vec![mapper])],
    )
}
pub fn fetch_symbol_max_leverage<V, E>(h: &StreamOps<V, E>) -> V {
    (h.number)(1.)
}
pub fn set_leverage<V: Clone, E>(h: &StreamOps<V, E>, symbol: V) -> Result<V, E> {
    let update = (h.capture)("this.store.update")?;
    let leverage = (h.object)(vec![]);
    (h.intrinsic)(
        "spread-into",
        vec![leverage.clone(), (h.capture)("this.store.fetchedLeverage")?],
    )?;
    let key = (h.intrinsic)("to-property-key", vec![symbol])?;
    let values = (h.object)(vec![
        ("buyLeverage", (h.number)(1.)),
        ("sellLeverage", (h.number)(1.)),
    ]);
    (h.intrinsic)("define-field", vec![leverage.clone(), key, values])?;
    (h.invoke)(
        update,
        vec![(h.object)(vec![("fetchedLeverage", leverage)])],
    )?;
    Ok((h.undefined)())
}
pub fn change_margin_type<V, E>(h: &StreamOps<V, E>) -> V {
    (h.undefined)()
}
pub fn change_position_mode<V, E>(h: &StreamOps<V, E>) -> V {
    (h.undefined)()
}
